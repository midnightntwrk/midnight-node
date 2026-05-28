use crate::{BlockByHash, LocalDataUnavailableReason, McHashDataSource, McHashInherentDigest};
use sidechain_domain::McBlockHash;
use sp_consensus::block_validation::{BlockAnnounceValidator, Validation};
use sp_partner_chains_consensus_aura::inherent_digest::InherentDigest;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{error::Error, future::Future, pin::Pin, sync::Arc};

/// Block announcement validator that preflights the announced main-chain hash digest.
///
/// When the digest references a Cardano block that is known locally, the announcement is
/// accepted even if the local Cardano tip looks stale. When the hash is unknown and local
/// Cardano observability is stale or unavailable, validation returns an internal error;
/// Polkadot SDK maps that to `Skip`, avoiding peer reputation penalties for a local db-sync lag.
/// If local Cardano observability is healthy and the hash is still unknown, the announcement is
/// treated as invalid without requesting an immediate disconnect.
#[derive(Clone)]
pub struct McHashBlockAnnounceValidator {
	block_source: Arc<dyn McHashDataSource + Send + Sync>,
}

impl McHashBlockAnnounceValidator {
	/// Creates a validator using the provided main-chain hash data source.
	pub fn new(block_source: Arc<dyn McHashDataSource + Send + Sync>) -> Self {
		Self { block_source }
	}
}

#[derive(Debug, thiserror::Error)]
enum McHashBlockAnnounceValidationError {
	#[error("local Cardano data unavailable while validating announced MC hash {hash}: {reason}")]
	LocalDataUnavailable { hash: McBlockHash, reason: LocalDataUnavailableReason },
	#[error(
		"failed to query local Cardano data while validating announced MC hash {hash}: {error}"
	)]
	DataSourceError { hash: McBlockHash, error: String },
	#[error("local Cardano view is not healthy while validating announced MC hash {0}")]
	CardanoNotOk(McBlockHash),
}

impl<B> BlockAnnounceValidator<B> for McHashBlockAnnounceValidator
where
	B: BlockT,
{
	fn validate(
		&mut self,
		header: &B::Header,
		data: &[u8],
	) -> Pin<Box<dyn Future<Output = Result<Validation, Box<dyn Error + Send>>> + Send>> {
		if !data.is_empty() {
			log::debug!("Received unknown data alongside the MC hash block announcement.",);
			return Box::pin(async { Ok(Validation::Failure { disconnect: true }) });
		}

		let mc_hash = match McHashInherentDigest::value_from_digest(header.digest().logs()) {
			Ok(mc_hash) => mc_hash,
			Err(err) => {
				log::debug!("Failed to retrieve MC hash from announced block digest: {err}");
				return Box::pin(async { Ok(Validation::Failure { disconnect: true }) });
			},
		};
		let block_source = self.block_source.clone();

		Box::pin(async move {
			match block_source.get_block_by_hash(mc_hash.clone()).await {
				Ok(BlockByHash::Found(_)) => Ok(Validation::Success { is_new_best: false }),
				Ok(BlockByHash::LocalDataUnavailable { reason }) => {
					Err(Box::new(McHashBlockAnnounceValidationError::LocalDataUnavailable {
						hash: mc_hash,
						reason,
					}) as Box<dyn Error + Send>)
				},
				Ok(BlockByHash::NotFound { .. }) => {
					let is_cardano_tip_fresh =
						block_source.is_cardano_tip_fresh().await.map_err(|error| {
							Box::new(McHashBlockAnnounceValidationError::DataSourceError {
								hash: mc_hash.clone(),
								error: error.to_string(),
							}) as Box<dyn Error + Send>
						})?;
					if !is_cardano_tip_fresh {
						return Err(Box::new(McHashBlockAnnounceValidationError::CardanoNotOk(
							mc_hash,
						)) as Box<dyn Error + Send>);
					}

					let is_cardano_ok = block_source.is_cardano_ok().await.map_err(|error| {
						Box::new(McHashBlockAnnounceValidationError::DataSourceError {
							hash: mc_hash.clone(),
							error: error.to_string(),
						}) as Box<dyn Error + Send>
					})?;

					if is_cardano_ok {
						Ok(Validation::Failure { disconnect: false })
					} else {
						Err(Box::new(McHashBlockAnnounceValidationError::CardanoNotOk(mc_hash))
							as Box<dyn Error + Send>)
					}
				},
				Err(error) => Err(Box::new(McHashBlockAnnounceValidationError::DataSourceError {
					hash: mc_hash,
					error: error.to_string(),
				}) as Box<dyn Error + Send>),
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		LatestStableBlockForTimestamp, MC_HASH_DIGEST_ID, MainchainBlock, StableBlockForHash,
	};
	use sidechain_domain::{McBlockNumber, McEpochNumber, McSlotNumber};
	use sp_runtime::OpaqueExtrinsic;
	use sp_runtime::testing::{Digest, Header};
	use sp_timestamp::Timestamp;
	use std::io;

	type Block = sp_runtime::generic::Block<Header, OpaqueExtrinsic>;

	#[derive(Clone, Copy)]
	enum Query<T> {
		Value(T),
		Error(&'static str),
		Panic(&'static str),
	}

	#[derive(Clone, Copy)]
	enum BlockLookup {
		Found,
		LocalDataUnavailable,
		NotFound,
		Error,
		Panic,
	}

	struct TestDataSource {
		tip_fresh: Query<bool>,
		block_lookup: BlockLookup,
		cardano_ok: Query<bool>,
	}

	impl Default for TestDataSource {
		fn default() -> Self {
			Self {
				tip_fresh: Query::Value(true),
				block_lookup: BlockLookup::Found,
				cardano_ok: Query::Value(true),
			}
		}
	}

	impl TestDataSource {
		fn no_queries_expected() -> Self {
			Self {
				tip_fresh: Query::Panic("validator should not query tip freshness"),
				block_lookup: BlockLookup::Panic,
				cardano_ok: Query::Panic("validator should not query Cardano health"),
			}
		}
	}

	#[async_trait::async_trait]
	impl McHashDataSource for TestDataSource {
		async fn get_latest_stable_block_for(
			&self,
			_reference_timestamp: Timestamp,
		) -> Result<LatestStableBlockForTimestamp, Box<dyn Error + Send + Sync>> {
			unreachable!("block announce validator does not query latest stable block")
		}

		async fn get_stable_block_for(
			&self,
			_hash: McBlockHash,
			_reference_timestamp: Timestamp,
		) -> Result<StableBlockForHash, Box<dyn Error + Send + Sync>> {
			unreachable!("block announce validator does not query stable block by hash")
		}

		async fn get_block_by_hash(
			&self,
			hash: McBlockHash,
		) -> Result<BlockByHash, Box<dyn Error + Send + Sync>> {
			match self.block_lookup {
				BlockLookup::Found => Ok(BlockByHash::Found(mainchain_block(hash))),
				BlockLookup::LocalDataUnavailable => Ok(BlockByHash::LocalDataUnavailable {
					reason: LocalDataUnavailableReason::LatestBlockUnavailable,
				}),
				BlockLookup::NotFound => Ok(BlockByHash::NotFound { hash }),
				BlockLookup::Error => Err(Box::new(io::Error::other("block lookup failed"))),
				BlockLookup::Panic => unreachable!("validator should not query block by hash"),
			}
		}

		async fn is_cardano_tip_fresh(&self) -> Result<bool, Box<dyn Error + Send + Sync>> {
			query_bool(self.tip_fresh)
		}

		async fn is_cardano_ok(&self) -> Result<bool, Box<dyn Error + Send + Sync>> {
			query_bool(self.cardano_ok)
		}
	}

	#[tokio::test]
	async fn accepts_announcement_when_mc_hash_is_known() {
		let mut validator = validator(TestDataSource::default());

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("known MC hash should be valid");

		assert_eq!(result, Validation::Success { is_new_best: false });
	}

	#[tokio::test]
	async fn accepts_announcement_when_mc_hash_is_known_even_if_tip_is_not_fresh() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Value(false),
			block_lookup: BlockLookup::Found,
			cardano_ok: Query::Panic("known hash should not query Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("known MC hash should be valid even when tip looks stale");

		assert_eq!(result, Validation::Success { is_new_best: false });
	}

	#[tokio::test]
	async fn skips_announcement_when_mc_hash_is_unknown_and_tip_is_not_fresh() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Value(false),
			block_lookup: BlockLookup::NotFound,
			cardano_ok: Query::Panic("stale tip should skip before checking Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_mc_hash_is_unknown_after_fresh_tip_check() {
		let mut validator = validator(TestDataSource {
			block_lookup: BlockLookup::NotFound,
			cardano_ok: Query::Value(false),
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn rejects_announcement_when_mc_hash_is_unknown_and_cardano_is_ok() {
		let mut validator = validator(TestDataSource {
			block_lookup: BlockLookup::NotFound,
			cardano_ok: Query::Value(true),
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[])
			.await
			.expect("healthy Cardano view can classify unknown hash");

		assert_eq!(result, Validation::Failure { disconnect: false });
	}

	#[tokio::test]
	async fn rejects_announcement_with_non_empty_announce_data() {
		let mut validator = validator(TestDataSource::no_queries_expected());

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[1])
			.await
			.expect("non-empty announce data is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn rejects_announcement_with_missing_mc_hash_digest() {
		let mut validator = validator(TestDataSource::no_queries_expected());
		let header = Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: vec![] },
		);

		let result = validate(&mut validator, &header, &[])
			.await
			.expect("missing MC hash digest is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn rejects_announcement_with_malformed_mc_hash_digest() {
		let mut validator = validator(TestDataSource::no_queries_expected());
		let header = Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest {
				logs: vec![sp_runtime::DigestItem::PreRuntime(MC_HASH_DIGEST_ID, vec![7; 31])],
			},
		);

		let result = validate(&mut validator, &header, &[])
			.await
			.expect("malformed MC hash digest is a peer validation failure");

		assert_eq!(result, Validation::Failure { disconnect: true });
	}

	#[tokio::test]
	async fn skips_announcement_when_tip_freshness_query_fails_for_unknown_hash() {
		let mut validator = validator(TestDataSource {
			tip_fresh: Query::Error("tip freshness query failed"),
			block_lookup: BlockLookup::NotFound,
			cardano_ok: Query::Panic("tip freshness error should skip before Cardano health"),
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_block_lookup_reports_local_data_unavailable() {
		let mut validator = validator(TestDataSource {
			block_lookup: BlockLookup::LocalDataUnavailable,
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_block_lookup_fails() {
		let mut validator =
			validator(TestDataSource { block_lookup: BlockLookup::Error, ..Default::default() });

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn skips_announcement_when_cardano_ok_query_fails_after_unknown_hash() {
		let mut validator = validator(TestDataSource {
			block_lookup: BlockLookup::NotFound,
			cardano_ok: Query::Error("Cardano health query failed"),
			..Default::default()
		});

		let result = validate(&mut validator, &mock_header(McBlockHash([7; 32])), &[]).await;

		assert!(result.is_err(), "validator errors are mapped to Skip by the SDK");
	}

	#[tokio::test]
	async fn uses_first_mc_hash_digest_when_duplicates_are_present() {
		let known_hash = McBlockHash([7; 32]);
		let unknown_hash = McBlockHash([8; 32]);
		let mut validator = validator(TestDataSource::default());
		let header = Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest {
				logs: [
					McHashInherentDigest::from_mc_block_hash(known_hash),
					McHashInherentDigest::from_mc_block_hash(unknown_hash),
				]
				.concat(),
			},
		);

		let result = validate(&mut validator, &header, &[])
			.await
			.expect("current digest parser uses the first matching MC hash");

		assert_eq!(result, Validation::Success { is_new_best: false });
	}

	async fn validate(
		validator: &mut McHashBlockAnnounceValidator,
		header: &Header,
		data: &[u8],
	) -> Result<Validation, Box<dyn Error + Send>> {
		<McHashBlockAnnounceValidator as BlockAnnounceValidator<Block>>::validate(
			validator, header, data,
		)
		.await
	}

	fn validator(source: TestDataSource) -> McHashBlockAnnounceValidator {
		McHashBlockAnnounceValidator::new(Arc::new(source))
	}

	fn query_bool(query: Query<bool>) -> Result<bool, Box<dyn Error + Send + Sync>> {
		match query {
			Query::Value(value) => Ok(value),
			Query::Error(message) => Err(Box::new(io::Error::other(message))),
			Query::Panic(message) => unreachable!("{message}"),
		}
	}

	fn mock_header(mc_hash: McBlockHash) -> Header {
		Header::new(
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: McHashInherentDigest::from_mc_block_hash(mc_hash) },
		)
	}

	fn mainchain_block(hash: McBlockHash) -> MainchainBlock {
		MainchainBlock {
			number: McBlockNumber(1),
			hash,
			epoch: McEpochNumber(1),
			slot: McSlotNumber(1),
			timestamp: 1,
		}
	}
}
